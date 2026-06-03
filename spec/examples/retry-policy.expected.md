# Overview

Retries are capped at **three attempts**. See [the gateway guide](https://example.com/gateway) and the [Methods](krg:///h_meth) section.

> Retries are only safe for idempotent requests.

# Methods

The backoff schedule is exponential:

1. First retry after 1s.
2. Second retry after 2s, then:
   - with full jitter applied.

```go
func backoff(n int) time.Duration {
	return (1 << n) * time.Second
}
```

## Results

| Attempt | Delay |
| :--- | ---: |
| 1 | 1s |

![p95 latency by attempt](media/latency.png)

*Latency rises with each attempt*

---

:::acme:callout{variant="warn"}
Never retry on 4xx responses.
:::
