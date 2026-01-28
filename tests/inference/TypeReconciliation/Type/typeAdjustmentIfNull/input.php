<?php
class A {}
class B {}

$var = rand(0,10) > 5 ? new A : null;

if ($var === null) {
    $var = new B;
}