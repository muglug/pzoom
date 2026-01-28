<?php
class Bar {
    public function getFoo(): string {
        return "foo";
    }
}

/**
 * @template TKey
 * @template T
 */
interface Collection {
    /**
     * @param Closure(T=):bool $p
     * @return Collection<TKey, T>
     */
    public function filter(Closure $p);
}

/**
 * @param Collection<int, Bar> $c
 * @psalm-return Collection<int, Bar>
 */
function filter(Collection $c, string $name) {
    return $c->filter(
        function (Bar $f) use ($name) {
            return $f->getFoo() === "foo";
        }
    );
}