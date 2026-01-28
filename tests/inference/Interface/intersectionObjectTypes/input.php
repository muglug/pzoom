<?php

class C {}

interface IFoo {
    function foo() : object;
}

interface IBar {
    function foo() : C;
}

/** @param IFoo&IBar $i */
function iFooFirst($i) : C {
    return $i->foo();
}

/** @param IBar&IFoo $i */
function iBarFirst($i) : C {
    return $i->foo();
}
