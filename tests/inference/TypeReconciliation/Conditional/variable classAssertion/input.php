<?php
abstract class A {}
class B extends A {}

function a(A $a): void {
    if($a::class == B::class) {
        b($a);
    }
}

function b(B $_b): void {
}