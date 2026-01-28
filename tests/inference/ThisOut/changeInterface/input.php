<?php
interface Foo {
    /**
     * @return void
     */
    public function far() {
    }
}
class Bar {
    /**
     * @psalm-this-out Foo
     * @return void
     */
    public function baz() {
    }
}
$bar = new Bar();
$bar->baz();
$bar->far();
