<?php
/**
 * @template A
 * @template B
 * @template C
 */
final class Processor
{
    /**
     * @param A $a
     * @param B $b
     * @param C $c
     */
    public function __construct(
        public readonly mixed $a,
        public readonly mixed $b,
        public readonly mixed $c,
    ) {}

    /**
     * @template AProcessed
     * @template BProcessed
     * @template CProcessed
     * @param callable(A, B, C): list{AProcessed, BProcessed, CProcessed} $processAB
     * @return list{AProcessed, BProcessed, CProcessed}
     */
    public function process(callable $processAB): array
    {
        return $processAB($this->a, $this->b, $this->c);
    }
}

/**
 * @template A
 * @template B
 * @template C
 * @param A $value1
 * @param B $value2
 * @param C $value3
 * @return list{A, B, C}
 */
function tripleId(mixed $value1, mixed $value2, mixed $value3): array
{
    return [$value1, $value2, $value3];
}

$processor = new Processor(a: 1, b: 2, c: 3);

$test = $processor->process(tripleId(...));
