<?php
/** @template T */
interface A {
    /** @return T */
    public function output();
}

/**
 * @psalm-type Foo=string
 * @implements A<Foo>
 */
class C implements A {
    public function output() {
        return "hello";
    }
}

$instance = new C();
$output = $instance->output();
