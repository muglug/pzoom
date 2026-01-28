<?php
interface A {
}
function foo (A&string $test): A&string {
    return $test;
}
