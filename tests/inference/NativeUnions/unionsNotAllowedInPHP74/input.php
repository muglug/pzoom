<?php
interface A {
}
interface B {
}
function foo (A|B $test): A&B {
    return $test;
}
