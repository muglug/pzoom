<?php
/** @template T of int */
class IntBox {
    public function __construct() {}

    /** @param T $item */
    public function add($item): void {}
}

/** @psalm-suppress IncompatibleTypeParameters */
function probe(): void {
    $box = new IntBox();
    $box->add("nope");
}
