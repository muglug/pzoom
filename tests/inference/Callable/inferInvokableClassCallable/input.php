<?php
/**
 * @template A
 * @template B
 */
final class MapOperator
{
    /** @var Closure(A): B */
    private Closure $ab;

    /**
     * @param callable(A): B $ab
     */
    public function __construct(callable $ab)
    {
        $this->ab = Closure::fromCallable($ab);
    }

    /**
     * @template K
     * @param array<K, A> $a
     * @return array<K, B>
     */
    public function __invoke(array $a): array
    {
        $b = [];

        foreach ($a as $k => $v) {
            $b[$k] = ($this->ab)($v);
        }

        return $b;
    }
}
/**
 * @template A
 * @template B
 * @param A $a
 * @param callable(A): B $ab
 * @return B
 */
function pipe(mixed $a, callable $ab): mixed
{
    return $ab($a);
}
/**
 * @return array<string, int>
 */
function getDict(): array
{
    return ["fst" => 1, "snd" => 2, "thr" => 3];
}
$result = pipe(getDict(), new MapOperator(fn($i) => ["num" => $i]));
