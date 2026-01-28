<?php
/** @param mixed $x */
function myvalue($x): void {
    /** @var $myVar MyNS\OtherClass */
    $myVar = $x->conn()->method();
    $myVar->otherMethod();
}
