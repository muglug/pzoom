<?php
/**  @template T  */
interface Aggregator
{
    /**
     * @psalm-param T ...$values
     * @psalm-return T
     */
    public function aggregate(...$values): mixed;
}

/** @implements Aggregator<int|float|null> */
final class AverageAggregator implements Aggregator
{
    public function aggregate(...$values): null|int|float
    {
        if (!$values) {
            return null;
        }
        return array_sum($values) / count($values);
    }
}
