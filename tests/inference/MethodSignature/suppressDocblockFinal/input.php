<?php
/**
 * @final
 */
class A {
   public function foo(): void {}
}

/**
 * @psalm-suppress InvalidExtendClass
 */
class B extends A {
    /**
     * @psalm-suppress MethodSignatureMismatch
     */
    public function foo(): void {}
}
