# Raw Experimental Data

This directory contains the raw data from all MAESTRO experiments.

## File Structure

- `maestro_results.csv` - Aggregated results from all 52 circuits
- `maestro_results_sample.csv` - Sample data showing the format
- Individual circuit results: `{circuit_name}_results.csv`

## Data Format

Each CSV file contains the following columns:

| Column | Type | Description |
|--------|------|-------------|
| timestamp | float | Unix timestamp of the test |
| circuit | string | Circuit name (e.g., "Series-5-LEDs") |
| category | string | Circuit category (series/parallel/power/etc.) |
| solver | string | Solver used (Newton-Raphson/GLACIER/MAESTRO/MAESTRO+GLACIER) |
| converged | boolean | Whether the solver converged |
| iterations | integer | Number of iterations (if converged) |
| time_ms | float | Solution time in milliseconds |
| residual | float | Final residual norm |
| strategy | string | Strategy used (for MAESTRO) |
| notes | string | Additional notes or failure reasons |

## Usage

### Loading in Python
```python
import pandas as pd

# Load all results
df = pd.read_csv('maestro_results.csv')

# Filter by solver
maestro_results = df[df['solver'] == 'MAESTRO']

# Calculate success rate
success_rate = df.groupby('solver')['converged'].mean()
```

### Loading in R
```r
# Load data
data <- read.csv('maestro_results.csv')

# Summary statistics
summary(data)

# Success rate by category
aggregate(converged ~ solver + category, data, mean)
```

## Data Integrity

- All timestamps are UTC
- Missing values indicate solver failure before metric collection
- Residuals are L2 norms of the equation system
- Times include setup but exclude result validation

## Reproducing the Data

To regenerate all raw data:
```bash
cd ../Code_Repository/benchmarks
./run_all_experiments.sh
```

This will overwrite existing data files.

## License

This data is provided under the same license as the MAESTRO paper.
Please cite the paper if you use this data in your research.