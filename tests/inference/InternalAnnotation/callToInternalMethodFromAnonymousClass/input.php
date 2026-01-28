<?php
namespace X;

/**
 * @internal
 * @psalm-internal X
 */
class A
{
    public function a(): void {}
}

new class (new A)
{
    public function __construct(
        private A $a
    ) {
        $a->a();
    }
};
