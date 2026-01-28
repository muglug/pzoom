<?php
interface A {
    function foo(): void;
}
interface B {
}
class C implements A, B {
    function foo(): void {
    }
}
function test(A&B $in): void {
    $in->foo();
}
test(new C());
                
