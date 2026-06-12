<?php
/**
 * @template TIn
 * @template TOut
 */
final class Map
{
    /** @param Closure(TIn): TOut $c */
    public function __construct(private Closure $c) {}

    /**
     * @template TIn2 as list<TIn>
     * @param TIn2 $in
     * @return list<TOut>
     */
    public function __invoke(array $in) : array {
        return array_map(
            $this->c,
            $in
        );
    }
}

$m = new Map(fn(int $num) => (string) $num);
$m(["a"]);
