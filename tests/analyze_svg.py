#!/usr/bin/env python3
"""
SVG Circuit Analyzer - Identifies rendering issues in generated circuit schematics
"""

import xml.etree.ElementTree as ET
import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from typing import List, Tuple, Optional

@dataclass
class BoundingBox:
    min_x: float
    min_y: float
    max_x: float
    max_y: float

    @property
    def width(self):
        return self.max_x - self.min_x

    @property
    def height(self):
        return self.max_y - self.min_y

    @property
    def center(self):
        return ((self.min_x + self.max_x) / 2, (self.min_y + self.max_y) / 2)

    def overlaps(self, other: 'BoundingBox', margin: float = 5.0) -> bool:
        """Check if two bounding boxes overlap with margin"""
        return not (self.max_x + margin < other.min_x or
                   self.min_x > other.max_x + margin or
                   self.max_y + margin < other.min_y or
                   self.min_y > other.max_y + margin)

    def contains_point(self, x: float, y: float) -> bool:
        return self.min_x <= x <= self.max_x and self.min_y <= y <= self.max_y

@dataclass
class Component:
    label: str
    position: Tuple[float, float]
    bbox: BoundingBox
    symbol_type: str
    transform: str

@dataclass
class Issue:
    severity: str  # ERROR, WARNING, INFO
    category: str
    message: str
    details: Optional[str] = None

class SVGAnalyzer:
    def __init__(self, svg_path: str):
        self.svg_path = svg_path
        self.tree = ET.parse(svg_path)
        self.root = self.tree.getroot()
        self.ns = {'svg': 'http://www.w3.org/2000/svg'}
        self.issues: List[Issue] = []
        self.components: List[Component] = []

    def analyze(self):
        """Run all analysis checks"""
        print(f"\n{'='*80}")
        print(f"SVG ANALYSIS REPORT: {self.svg_path}")
        print(f"{'='*80}\n")

        self.check_viewbox()
        self.extract_components()
        self.check_component_overlap()
        self.check_component_visibility()
        self.check_labels()
        self.check_nets()
        self.check_transforms()

        return self.print_report()

    def check_viewbox(self):
        """Check if viewBox is appropriate for content"""
        viewbox_str = self.root.get('viewBox')
        width_str = self.root.get('width')
        height_str = self.root.get('height')

        if not viewbox_str:
            self.issues.append(Issue('ERROR', 'ViewBox', 'No viewBox attribute found'))
            return

        try:
            vb_parts = [float(x) for x in viewbox_str.split()]
            vb_x, vb_y, vb_w, vb_h = vb_parts

            print(f"📐 ViewBox: [{vb_x}, {vb_y}] size: {vb_w} × {vb_h}")
            print(f"📏 Canvas size: {width_str} × {height_str}")

            # Check if viewBox is reasonable
            if vb_w < 10 or vb_h < 10:
                self.issues.append(Issue('ERROR', 'ViewBox',
                    f'ViewBox too small: {vb_w} × {vb_h}'))

            if vb_w > 10000 or vb_h > 10000:
                self.issues.append(Issue('WARNING', 'ViewBox',
                    f'ViewBox very large: {vb_w} × {vb_h}'))

        except Exception as e:
            self.issues.append(Issue('ERROR', 'ViewBox',
                f'Invalid viewBox format: {viewbox_str}', str(e)))

    def extract_components(self):
        """Extract all components with their positions and labels"""
        # Find all component groups (g elements with transform)
        for g in self.root.findall('.//svg:g[@transform]', self.ns):
            transform = g.get('transform')

            # Extract position from transform
            match = re.search(r'translate\(([-\d.]+),\s*([-\d.]+)\)', transform)
            if not match:
                continue

            x, y = float(match.group(1)), float(match.group(2))

            # Look for nested SVG (component symbol)
            nested_svg = g.find('.//svg:svg', self.ns)
            if nested_svg is None:
                continue

            # Get viewBox of nested SVG to determine size
            nested_viewbox = nested_svg.get('viewBox', '0 0 20 20')
            vb_parts = [float(v) for v in nested_viewbox.split()]

            # Extract symbol type from text elements
            symbol_type = "Unknown"
            for text in nested_svg.findall('.//svg:text', self.ns):
                if text.text and len(text.text) < 30:  # Short text = symbol name
                    symbol_type = text.text
                    break

            # Create bounding box for component
            bbox = BoundingBox(
                x + vb_parts[0], y + vb_parts[1],
                x + vb_parts[0] + vb_parts[2], y + vb_parts[1] + vb_parts[3]
            )

            # Find label (next text element after g)
            label = "Unknown"
            next_elem = None
            for elem in self.root.iter():
                if elem == g:
                    # Get next element
                    parent = self.root
                    for p in self.root.iter():
                        for child in p:
                            if child == g:
                                parent = p
                                break

                    siblings = list(parent)
                    g_index = siblings.index(g)
                    if g_index + 1 < len(siblings):
                        next_elem = siblings[g_index + 1]
                        if next_elem.tag.endswith('text'):
                            label = next_elem.text or "Unknown"
                    break

            component = Component(label, (x, y), bbox, symbol_type, transform)
            self.components.append(component)

        print(f"\n🔧 Found {len(self.components)} components:")
        for i, comp in enumerate(self.components, 1):
            print(f"  {i}. {comp.label:20s} @ ({comp.position[0]:7.1f}, {comp.position[1]:7.1f}) "
                  f"Symbol: {comp.symbol_type}")

    def check_component_overlap(self):
        """Check if components are overlapping"""
        overlaps = []
        for i, comp1 in enumerate(self.components):
            for j, comp2 in enumerate(self.components[i+1:], i+1):
                if comp1.bbox.overlaps(comp2.bbox):
                    distance = ((comp1.position[0] - comp2.position[0])**2 +
                               (comp1.position[1] - comp2.position[1])**2)**0.5
                    overlaps.append((comp1.label, comp2.label, distance))

        if overlaps:
            print(f"\n⚠️  Component Overlaps: {len(overlaps)}")
            for c1, c2, dist in overlaps[:5]:  # Show first 5
                self.issues.append(Issue('ERROR', 'Overlap',
                    f'Components overlap: {c1} and {c2}',
                    f'Distance: {dist:.1f}'))
                print(f"  - {c1} ↔ {c2} (distance: {dist:.1f})")
        else:
            print(f"\n✓ No component overlaps detected")

    def check_component_visibility(self):
        """Check if all components are within viewBox"""
        viewbox_str = self.root.get('viewBox')
        if not viewbox_str:
            return

        vb_parts = [float(x) for x in viewbox_str.split()]
        vb_bbox = BoundingBox(vb_parts[0], vb_parts[1],
                              vb_parts[0] + vb_parts[2],
                              vb_parts[1] + vb_parts[3])

        outside = []
        for comp in self.components:
            if not vb_bbox.contains_point(*comp.position):
                outside.append(comp.label)
                self.issues.append(Issue('ERROR', 'Visibility',
                    f'Component outside viewBox: {comp.label}',
                    f'Position: {comp.position}'))

        if outside:
            print(f"\n⚠️  Components outside viewBox: {len(outside)}")
            for label in outside[:5]:
                print(f"  - {label}")
        else:
            print(f"\n✓ All components visible in viewBox")

    def check_labels(self):
        """Check component labels"""
        labels_found = 0
        labels_missing = 0

        for text in self.root.findall('.//svg:text[@class="component-text"]', self.ns):
            if text.text:
                labels_found += 1
            else:
                labels_missing += 1

        print(f"\n🏷️  Labels: {labels_found} found, {labels_missing} missing")

        if labels_missing > 0:
            self.issues.append(Issue('WARNING', 'Labels',
                f'{labels_missing} components have missing labels'))

    def check_nets(self):
        """Check net routing"""
        nets = self.root.findall('.//svg:line[@class]', self.ns)
        net_types = defaultdict(int)

        for net in nets:
            net_class = net.get('class', '')
            if 'net' in net_class:
                net_types[net_class] += 1

        print(f"\n🔌 Nets found:")
        total_nets = 0
        for net_type, count in sorted(net_types.items()):
            print(f"  - {net_type}: {count}")
            total_nets += count

        if total_nets == 0:
            self.issues.append(Issue('WARNING', 'Nets',
                'No net connections found'))

    def check_transforms(self):
        """Check for malformed transforms"""
        for g in self.root.findall('.//svg:g[@transform]', self.ns):
            transform = g.get('transform')

            # Check for NaN or Inf
            if 'nan' in transform.lower() or 'inf' in transform.lower():
                self.issues.append(Issue('ERROR', 'Transform',
                    'Invalid transform with NaN/Inf', transform))

            # Check for excessive values
            numbers = re.findall(r'[-\d.]+', transform)
            for num_str in numbers:
                try:
                    num = float(num_str)
                    if abs(num) > 100000:
                        self.issues.append(Issue('WARNING', 'Transform',
                            f'Very large transform value: {num}', transform))
                except:
                    pass

    def print_report(self):
        """Print summary report"""
        print(f"\n{'='*80}")
        print(f"ISSUES SUMMARY")
        print(f"{'='*80}\n")

        if not self.issues:
            print("✅ No issues found!\n")
            return 0

        errors = [i for i in self.issues if i.severity == 'ERROR']
        warnings = [i for i in self.issues if i.severity == 'WARNING']
        info = [i for i in self.issues if i.severity == 'INFO']

        print(f"🔴 Errors: {len(errors)}")
        print(f"🟡 Warnings: {len(warnings)}")
        print(f"🔵 Info: {len(info)}")
        print()

        for issue in errors:
            print(f"🔴 ERROR [{issue.category}]: {issue.message}")
            if issue.details:
                print(f"   → {issue.details}")

        for issue in warnings:
            print(f"🟡 WARNING [{issue.category}]: {issue.message}")
            if issue.details:
                print(f"   → {issue.details}")

        for issue in info:
            print(f"🔵 INFO [{issue.category}]: {issue.message}")
            if issue.details:
                print(f"   → {issue.details}")

        print(f"\n{'='*80}\n")

        # Return exit code based on errors
        return len(errors)

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 analyze_svg.py <svg_file>")
        sys.exit(1)

    svg_path = sys.argv[1]
    analyzer = SVGAnalyzer(svg_path)
    error_count = analyzer.analyze()

    sys.exit(min(error_count, 1))

if __name__ == '__main__':
    main()
