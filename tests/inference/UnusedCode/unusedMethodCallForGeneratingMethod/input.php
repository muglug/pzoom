<?php
/**
 * @psalm-external-mutation-free
 */
final class A {
    private string $foo;

    public function __construct(string $foo) {
        $this->foo = $foo;
    }

    public function getFoo() : string {
        return "abular" . $this->foo;
    }
}

/**
 * @psalm-pure
 */
function makeA(string $s) : A {
    return new A($s);
}

function foo() : void {
    makeA("hello")->getFoo();
}
