<?php
/** @template T */
abstract class Collection {
    /** @param T $elem */
    public function add($elem): void {}
}

/**
 * @template T
 * @param Collection<T> $col
 */
function usesCollection(Collection $col): void {
    $col->add([]);
}
