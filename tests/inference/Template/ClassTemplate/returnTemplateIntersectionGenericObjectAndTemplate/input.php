<?php
/** @psalm-template Tp */
interface I {
    /** @psalm-return Tp */
    function getMe();
}

class C {}

/**
 * @psalm-template T as object
 *
 * @psalm-param class-string<T> $className
 *
 * @psalm-return T&I<T>
 *
 * @psalm-suppress MissingTemplateParam
 */
function makeConcrete(string $className) : object
{
    /** @var T&I<T> */
    return new class() extends C implements I {
        public function getMe() {
            return $this;
        }
    };
}

$a = makeConcrete(C::class);
