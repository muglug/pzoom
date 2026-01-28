<?php
/**
 * @template TKey of ?array-key
 * @template T
 */
interface Collection
{
    /**
     * @param Closure(T=):bool $p
     * @return Collection<TKey, T>
     */
    public function filter(Closure $p);
}