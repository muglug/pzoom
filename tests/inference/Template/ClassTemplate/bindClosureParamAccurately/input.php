<?php
/**
 * @template TKey
 * @template TValue
 */
interface Collection {
    /**
     * @template T
     * @param Closure(TValue):T $func
     * @return Collection<TKey,T>
     */
    public function map(Closure $func);

}

/**
 * @param Collection<int, string> $c
 */
function f(Collection $c): void {
    $fn = function(int $_p): bool { return true; };
    $c->map($fn);
}
