<?php
class B {
    public function foo() : void {}
}
/**
 * @param array<B> $b
 */
function foo(array $a, array $b) : void {
    $c = array_merge($b, $a);

    foreach ($c as $d) {
        $d->foo();
        if ($d instanceof B) {}
    }
}
