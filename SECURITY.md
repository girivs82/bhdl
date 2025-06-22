# Security Policy

## Supported Versions

Currently, BHDL is in active development. Security updates will be applied to:

| Version | Supported          |
| ------- | ------------------ |
| main    | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security vulnerability within BHDL, please follow these steps:

1. **DO NOT** file a public issue
2. Email your findings to [INSERT SECURITY EMAIL]
3. Include the following information:
   - Type of issue (e.g., buffer overflow, SQL injection, cross-site scripting, etc.)
   - Full paths of source file(s) related to the manifestation of the issue
   - The location of the affected source code (tag/branch/commit or direct URL)
   - Any special configuration required to reproduce the issue
   - Step-by-step instructions to reproduce the issue
   - Proof-of-concept or exploit code (if possible)
   - Impact of the issue, including how an attacker might exploit it

## Response Timeline

- Initial response: Within 48 hours
- Assessment: Within 7 days
- Fix development: Depends on complexity
- Public disclosure: After fix is released

## Security Considerations for BHDL

Given that BHDL processes hardware descriptions and can influence circuit safety:

### High Priority Issues
- Code execution vulnerabilities in the parser
- Path traversal in file operations
- Command injection in external tool integration
- Memory safety issues that could corrupt analysis results

### Medium Priority Issues
- Denial of service through malformed input
- Information disclosure through error messages
- Resource exhaustion

### Low Priority Issues
- Minor information leaks
- Non-exploitable crashes

## Security Best Practices

When using BHDL:

1. Only process BHDL files from trusted sources
2. Run BHDL tools in sandboxed environments when processing untrusted input
3. Keep your BHDL installation updated
4. Report suspicious behavior immediately

## Acknowledgments

We appreciate responsible disclosure and will acknowledge security researchers who help improve BHDL's security.