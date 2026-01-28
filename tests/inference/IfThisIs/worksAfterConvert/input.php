<?php
interface I {
    /**
     * @return void
     */
    public function test();
}

class F implements I
{
    /**
     * @psalm-this-out I
     * @return void
     */
    public function convert() {}

    /**
     * @psalm-if-this-is I
     * @return void
     */
    public function test() {}
}

$f = new F();
$f->convert();
$f->test();
