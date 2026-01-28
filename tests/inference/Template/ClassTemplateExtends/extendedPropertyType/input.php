<?php
interface I {}

/** @template T of I */
abstract class C {
    /** @var ?T */
    protected $m;
}

class Impl implements I {}

/** @template-extends C<Impl> */
class Test extends C {
    protected function foo() : void {
        $this->m = new Impl();
    }
}