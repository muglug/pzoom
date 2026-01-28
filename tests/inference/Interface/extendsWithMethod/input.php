<?php
interface A
{
    /**
     * @return string
     */
    public function fooFoo();
}

interface B extends A
{
    public function barBar() : void;
}

/** @return void */
function mux(B $b) {
    $b->fooFoo();
}
