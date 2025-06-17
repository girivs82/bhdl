# Multi-Backend Supplier Integration

## Overview

BHDL Components now supports multiple supplier API backends, making it accessible to individual developers who cannot obtain business-only API access. The system automatically handles fallbacks, rate limiting, and intelligent backend selection.

## Supported Backends

### 1. Nexar API (Recommended for Individual Developers)

**Access Level**: Free tier available  
**Quota**: 1,000 API calls per month  
**Registration**: Individual developers welcome  
**Requirements**: Valid credit card for verification (not charged on free tier)

```bash
# Set environment variables
export NEXAR_CLIENT_ID="your_client_id"
export NEXAR_CLIENT_SECRET="your_client_secret"

# Test the API
cargo run -p bhdl-components -- synthesize resistor \
  --enable-supplier-lookup \
  --supplier-backend nexar \
  --max-supplier-queries 5
```

**Setup Steps**:
1. Visit [Nexar Portal](https://nexar.com/api)
2. Sign up with personal email
3. Create an organization (can be personal name)
4. Generate API credentials
5. Add credit card for verification (free tier won't be charged)

### 2. DigiKey API

**Access Level**: Free for registered developers  
**Quota**: Varies by usage tier  
**Registration**: Individual developers accepted  
**Requirements**: Valid email and developer registration

```bash
# Set environment variables
export DIGIKEY_CLIENT_ID="your_client_id"
export DIGIKEY_CLIENT_SECRET="your_client_secret"

# Test the API
cargo run -p bhdl-components -- synthesize capacitor \
  --enable-supplier-lookup \
  --supplier-backend digikey \
  --max-supplier-queries 3
```

**Setup Steps**:
1. Visit [DigiKey API Portal](https://developer.digikey.com/)
2. Register as individual developer
3. Create application for component lookup
4. Get OAuth credentials

### 3. Multi-Backend Auto Mode (Default)

**Description**: Automatically tries multiple backends with intelligent fallback  
**Benefits**: Maximum data coverage and resilience

```bash
# Uses all available backends with automatic fallback
cargo run -p bhdl-components -- synthesize inductor \
  --enable-supplier-lookup \
  --supplier-backend auto \
  --max-supplier-queries 8
```

## Configuration

### Environment Variables

```bash
# Nexar API (Octopart successor)
export NEXAR_CLIENT_ID="your_nexar_client_id"
export NEXAR_CLIENT_SECRET="your_nexar_client_secret"

# DigiKey API
export DIGIKEY_CLIENT_ID="your_digikey_client_id"
export DIGIKEY_CLIENT_SECRET="your_digikey_client_secret"
```

### Backend Selection

| Backend | Best For | Free Tier | Setup Difficulty |
|---------|----------|-----------|------------------|
| `nexar` | General component search | 1,000 calls/month | Medium |
| `digikey` | DigiKey-specific parts | Varies | Easy |
| `auto` | Maximum coverage | Combined limits | Easy |

## API Rate Limiting Strategy

The system implements intelligent rate limiting to maximize the value of limited API quotas:

### Two-Stage Approach

1. **Stage 1**: Fast local database search (unlimited)
   - Filters 1000s of components by electrical specs
   - No API calls consumed

2. **Stage 2**: Targeted supplier lookups (limited API calls)
   - Only queries top candidates from Stage 1
   - Configurable limits (default: 10 calls max)
   - Real-time pricing and availability

### Smart Backend Selection

```rust
// Preferred order with health monitoring
preferred_backends: [
    SupplierBackend::Nexar,    // Free tier: 1000/month
    SupplierBackend::DigiKey,  // Free with registration
]

// Automatic fallback on:
// - API failures (3 consecutive)
// - Rate limit exceeded
// - Timeout (30s default)
```

## Usage Examples

### Basic Component Search (No API Calls)

```bash
# Spec-only search using local database
cargo run -p bhdl-components -- synthesize resistor \
  --requirements '{"resistance": 10000, "power_rating": 0.25, "quantity": 100}'
```

### Enhanced Search with Supplier Data

```bash
# With live pricing and availability
cargo run -p bhdl-components -- synthesize capacitor \
  --requirements '{"capacitance": 100e-9, "voltage_rating": 50, "quantity": 1000}' \
  --enable-supplier-lookup \
  --max-supplier-queries 5 \
  --supplier-backend auto
```

### Backend-Specific Search

```bash
# Force Nexar only
cargo run -p bhdl-components -- synthesize ic \
  --requirements '{"part_number": "LM358", "quantity": 100}' \
  --enable-supplier-lookup \
  --supplier-backend nexar

# Force DigiKey only  
cargo run -p bhdl-components -- synthesize resistor \
  --requirements '{"resistance": 1000, "quantity": 100}' \
  --enable-supplier-lookup \
  --supplier-backend digikey
```

## Quota Management

### Free Tier Limits

| Backend | Monthly Limit | Per Request | Recommended Usage |
|---------|---------------|-------------|-------------------|
| Nexar | 1,000 calls | 50 parts max | Daily prototyping |
| DigiKey | Varies | Single part | Specific component lookup |

### Best Practices

1. **Use spec-only mode for exploration**:
   ```bash
   # No API calls, fast local search
   --enable-supplier-lookup false
   ```

2. **Limit API queries for final selection**:
   ```bash
   # Only 3 API calls for top candidates
   --max-supplier-queries 3
   ```

3. **Cache results locally**:
   ```bash
   # Data cached for 4 hours by default
   # Subsequent searches use cache
   ```

## Error Handling

### Common Issues and Solutions

**Authentication Failed**:
```bash
Error: Authentication failed
Solution: Check API credentials in environment variables
```

**Quota Exceeded**:
```bash
Warning: Backend Nexar marked as unavailable after 3 failures
Solution: System automatically switches to DigiKey backend
```

**No API Access**:
```bash
# Graceful fallback to spec-only mode
cargo run -- synthesize resistor --requirements '{...}'
# Works without any API credentials
```

## Integration in BHDL Code

### Rust API Usage

```rust
use bhdl_components::supplier::multi_backend::{
    MultiBackendSupplierService, 
    MultiBackendConfig,
    SupplierBackend
};

// Configure for individual developer use
let config = MultiBackendConfig {
    preferred_backends: vec![
        SupplierBackend::Nexar,
        SupplierBackend::DigiKey,
    ],
    fallback_enabled: true,
    max_concurrent_requests: 2,
    ..Default::default()
};

// Create service
let mut supplier_service = MultiBackendSupplierService::new(config).await?;

// Search across all backends
let supplier_data = supplier_service
    .search_component_suppliers(&["LM358", "1N4148"])
    .await?;
```

### Two-Stage Synthesis Integration

```rust
use bhdl_components::synthesis::{TwoStageSynthesizer, TwoStageConfig};

let config = TwoStageConfig {
    enable_supplier_lookup: true,
    max_stage2_candidates: 5,  // Limit API usage
    supplier_cache_hours: 4,   // Short-term cache
    ..Default::default()
};

let synthesizer = TwoStageSynthesizer::new(config);
let result = synthesizer.synthesize(
    "resistor",
    &requirements,
    database,
    Some(&supplier_service)
).await?;
```

## Future Backends

Planned additions for individual developer access:

- **Arrow API**: Free tier available
- **Mouser API**: Registration-based access  
- **LCSC API**: Chinese supplier integration
- **Static Databases**: Offline component libraries

## Troubleshooting

### Check Backend Health

```bash
cargo run -p bhdl-components -- supplier stats
```

### Test Individual Backends

```bash
# Test Nexar
cargo run -p bhdl-components -- supplier update LM358 --backend nexar

# Test DigiKey  
cargo run -p bhdl-components -- supplier update LM358 --backend digikey
```

### Debug Mode

```bash
# Verbose logging
RUST_LOG=debug cargo run -p bhdl-components -- synthesize resistor \
  --enable-supplier-lookup --verbose
```

This multi-backend approach ensures BHDL Components remains accessible to individual developers while providing enterprise-grade supplier integration capabilities.