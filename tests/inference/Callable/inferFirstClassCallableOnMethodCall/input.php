<?php
/**
 * @template A
 * @template B
 */
final class Processor
{
    /**
     * @param A $a
     * @param B $b
     */
    public function __construct(
        public readonly mixed $a,
        public readonly mixed $b,
    ) {}

    /**
     * @template AProcessed
     * @template BProcessed
     * @param callable(A): AProcessed $processA
     * @param callable(B): BProcessed $processB
     * @return list{AProcessed, BProcessed}
     */
    public function process(callable $processA, callable $processB): array
    {
        return [$processA($this->a), $processB($this->b)];
    }
}

/**
 * @template A
 * @param A $value
 * @return A
 */
function id(mixed $value): mixed
{
    return $value;
}

function intToString(int $value): string
{
    return (string) $value;
}

/**
 * @template A
 * @param A $value
 * @return list{A}
 */
function singleToList(mixed $value): array
{
    return [$value];
}

$processor = new Processor(a: 1, b: 2);

$test_id = $processor->process(id(...), id(...));
$test_complex = $processor->process(intToString(...), singleToList(...));
