<?php
class B {}

$out = [];

if (rand(0,10) === 10) {
    $out[] = new B();
}
