<?php
/**
 * @template TKey
 * @template TValue
 */
class Collection {
    /**
     * @return array{0:Collection<TKey,TValue>,1:Collection<TKey,TValue>}
     * @psalm-suppress InvalidReturnType
     */
    public function partition() {}
}

/** @var Collection<int,string> $c */
$c = new Collection;
[$partA, $partB] = $c->partition();