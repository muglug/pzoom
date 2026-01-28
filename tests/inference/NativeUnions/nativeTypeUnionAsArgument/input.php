<?php
interface A {
    function foo(): void;
}
interface B {
    function foo(): void;
}
class C implements A {
    function foo(): void {
    }
}
function test(A|B $in): void {
    $in->foo();
}
test(new C());
                
