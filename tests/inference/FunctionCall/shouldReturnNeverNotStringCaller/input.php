<?php
/**
 * @return string
 */
function foo() {
   finalFunc();
}

/**
 * @return never
 */
function finalFunc() {
    exit;
}

foo();
