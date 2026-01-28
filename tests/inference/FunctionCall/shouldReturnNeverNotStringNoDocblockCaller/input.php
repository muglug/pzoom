<?php
/**
 * @return string
 */
function foo() {
   finalFunc();
}

function finalFunc() {
    exit;
}

foo();
