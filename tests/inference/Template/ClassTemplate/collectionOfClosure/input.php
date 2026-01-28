<?php
/**
 * @template TKey
 * @template TValue
 */
class Collection {
    /**
     * @param Closure(TValue):bool $p
     * @return Collection<TKey,TValue>
     */
    public function filter(Closure $p) {
        return $this;
    }
}
class I {}

/** @var Collection<mixed,Collection<mixed,I>> $c */
$c = new Collection;

$c->filter(
    /** @param Collection<mixed,I> $elt */
    function(Collection $elt): bool { return (bool) rand(0,1); }
);

$c->filter(
    /** @param Collection<mixed,I> $elt */
    function(Collection $elt): bool { return true; }
);