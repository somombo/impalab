import json
import sys
from typing import List, Dict, Any, Optional

from . import jsonl

class Lab:
    def __init__(self, results: str):
        self.results = jsonl.loads(results)

    def to_dataframe(self, flatten_attributes: bool = True, flatten_meta: bool = True):
        """Convert results to a pandas DataFrame."""
        try:
            import pandas as pd
        except ImportError as e:
            raise ImportError(
                "pandas is required to use to_dataframe(). "
                "Install it using: pip install pandas"
            ) from e

        rows = []
        string_attr_cols = set()
        for r in self.results:
            row = r.copy()
            
            executor = row.get("executor", "")
            args = row.get("args", [])
            if isinstance(args, list):
                args_str = " ".join(str(arg) for arg in args)
            else:
                args_str = str(args)
                
            row["task_label"] = f"{executor} {args_str}" if args_str else executor
            
            if flatten_attributes and "attributes" in row:
                attrs = row.get("attributes")
                if isinstance(attrs, dict):
                    row.pop("attributes")
                    for k, v in attrs.items():
                        col_name = f"attr.{k}"
                        row[col_name] = v
                        if isinstance(v, str):
                            string_attr_cols.add(col_name)
                        
            if flatten_meta and "gen_meta" in row:
                gen_m = row.get("gen_meta")
                if isinstance(gen_m, dict):
                    row.pop("gen_meta")
                    for k, v in gen_m.items():
                        row[f"gen.{k}"] = v
                        
            if flatten_meta and "exec_meta" in row:
                exec_m = row.get("exec_meta")
                if isinstance(exec_m, dict):
                    row.pop("exec_meta")
                    for k, v in exec_m.items():
                        row[f"exec.{k}"] = v
                        
            rows.append(row)
            
        df = pd.DataFrame(rows)
        for col in string_attr_cols:
            if col in df.columns:
                df[col] = df[col].astype("category")
        return df

    def summary(self, lower_is_better: bool = True):
        """Return a summary of statistics for each task."""
        try:
            import pandas as pd
            has_pandas = True
        except ImportError:
            has_pandas = False

        if has_pandas:
            df = self.to_dataframe()
            if df.empty:
                return pd.DataFrame()
            grouped = df.groupby("task_label")["metric"].agg(
                count="count",
                mean="mean",
                median="median",
                min="min",
                max="max",
                std="std"
            ).reset_index()
            grouped = grouped.sort_values(by="mean", ascending=lower_is_better)
            return grouped
        else:
            from collections import defaultdict
            import math
            
            task_metrics = defaultdict(list)
            for r in self.results:
                executor = r.get("executor", "")
                args = r.get("args", [])
                if isinstance(args, list):
                    args_str = " ".join(str(arg) for arg in args)
                else:
                    args_str = str(args)
                task_label = f"{executor} {args_str}" if args_str else executor
                
                metric = r.get("metric")
                if metric is not None:
                    try:
                        task_metrics[task_label].append(float(metric))
                    except (ValueError, TypeError):
                        pass
                    
            summary_list = []
            for label, metrics in task_metrics.items():
                if not metrics:
                    continue
                count = len(metrics)
                mean = sum(metrics) / count
                sorted_m = sorted(metrics)
                median = sorted_m[count // 2] if count % 2 != 0 else (sorted_m[count // 2 - 1] + sorted_m[count // 2]) / 2
                min_val = sorted_m[0]
                max_val = sorted_m[-1]
                variance = sum((x - mean) ** 2 for x in metrics) / max(1, count - 1)
                std = math.sqrt(variance)
                
                summary_list.append({
                    "task_label": label,
                    "count": count,
                    "mean": mean,
                    "median": median,
                    "min": min_val,
                    "max": max_val,
                    "std": std
                })
                
            summary_list.sort(key=lambda x: x["mean"], reverse=not lower_is_better)
            return summary_list

    def best(self, by: str = "mean", lower_is_better: bool = True) -> Dict[str, Any]:
        """Return the best performing task based on mean or other statistic."""
        stats = self.summary(lower_is_better=lower_is_better)
        if isinstance(stats, list):
            return stats[0] if stats else {}
        else:
            return stats.iloc[0].to_dict() if not stats.empty else {}

    def plot_distributions(self, metric_name: str = "metric", title: str = "Benchmark Results Distribution", ax=None):
        """Plot box plots of the metrics for each task."""
        try:
            import matplotlib.pyplot as plt
            import seaborn as sns
        except ImportError as e:
            raise ImportError(
                "matplotlib and seaborn are required for plotting. "
                "Install them using: pip install matplotlib seaborn"
            ) from e

        df = self.to_dataframe()
        
        if ax is None:
            _, ax = plt.subplots(figsize=(10, 6))
            
        sns.boxplot(data=df, x="task_label", y=metric_name, ax=ax)
        ax.set_title(title)
        ax.set_xlabel("Task")
        ax.set_ylabel(metric_name)
        plt.xticks(rotation=45, ha='right')
        plt.tight_layout()
        return ax

    def plot_bar(self, metric_name: str = "metric", title: str = "Average Benchmark Metrics", ax=None):
        """Plot a bar chart of the average metric for each task."""
        try:
            import matplotlib.pyplot as plt
            import seaborn as sns
        except ImportError as e:
            raise ImportError(
                "matplotlib and seaborn are required for plotting. "
                "Install them using: pip install matplotlib seaborn"
            ) from e

        df = self.to_dataframe()
        
        if ax is None:
            _, ax = plt.subplots(figsize=(10, 6))
            
        sns.barplot(data=df, x="task_label", y=metric_name, errorbar="sd", ax=ax)
        ax.set_title(title)
        ax.set_xlabel("Task")
        ax.set_ylabel(f"Average {metric_name}")
        plt.xticks(rotation=45, ha='right')
        plt.tight_layout()
        return ax

class LabFromResults(Lab):
    pass
