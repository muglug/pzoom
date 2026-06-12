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

function bad(): void {
    $box = new Box(1);
    $box->set(new DateTime());
}
