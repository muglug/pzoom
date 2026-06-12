<?php
/** @template T of int|string */
class Box {
    public function __construct() {}

    /** @param T $item */
    public function add($item): void {}
}

function good(): void {
    $box = new Box();
    $box->add(1);
    $box->add("two");
}

/** @template T */
class Holder {
    public function __construct() {}
}

function passesThrough(): Holder {
    return new Holder();
}
