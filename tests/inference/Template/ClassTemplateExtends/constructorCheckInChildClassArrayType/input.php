<?php
interface I {}

/** @template T of I */
abstract class C
{
    /** @var array<string, T> */
    protected $items = [];

    // added to trigger constructor initialisation checks
    // in descendant classes
    public int $i;

    /** @param array<string, T> $items */
    public function __construct($items = []) {
        $this->i = 5;

        foreach ($items as $k => $v) {
            $this->items[$k] = $v;
        }
    }
}

class Impl implements I {}

/**
 * @template-extends C<Impl>
 */
class Test extends C {}