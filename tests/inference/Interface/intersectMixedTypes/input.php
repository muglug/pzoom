<?php
interface IFoo {
    function foo() : string;
}

interface IBar {
    function foo() : string;
}

/** @param IFoo&IBar $i */
function iFooFirst($i) : string {
    return $i->foo();
}

/** @param IBar&IFoo $i */
function iBarFirst($i) : string {
    return $i->foo();
}
