<?php
/**
 * @template T of int|string
 */
class Box {
    /** @param T $t */
    public function __construct(public $t) {}
    /** @param T $item */
    public function set($item): void {
        $this->t = $item;
    }
}

/** @return Box<string> */
function bad(): Box {
    $box = new Box(1);
    $box->set("two");
    return $box;
}
