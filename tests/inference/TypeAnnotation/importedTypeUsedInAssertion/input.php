<?php
/** @psalm-type Foo = string */
class A {}

/**
 * @psalm-immutable
 * @psalm-import-type Foo from A as FooAlias
 */
class B {
    /**
     * @param mixed $input
     * @psalm-return FooAlias
     */
    public function convertToFoo($input) {
        $this->assertFoo($input);
        return $input;
    }

    /**
     * @param mixed $value
     * @psalm-assert FooAlias $value
     */
    private function assertFoo($value): void {
        if(!is_string($value)) {
            throw new \InvalidArgumentException();
        }
    }
}

$instance = new B();
$output = $instance->convertToFoo("hallo");
