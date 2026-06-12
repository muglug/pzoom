<?php
/** @template T of int */
class IntBox {
    public function __construct() {}

    /** @param T $item */
    public function add($item): void {}
}

$box = new IntBox();
$box->add("nope");
