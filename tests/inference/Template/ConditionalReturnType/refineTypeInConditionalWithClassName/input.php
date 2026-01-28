<?php
class A {}
class AChild extends A {}
class B {}

/**
 * @template TInput as object
 *
 * @param TInput $input
 *
 * @return (TInput is A ? TInput : A)
 */
function foobaz(object $input): A {
    if ($input instanceof A) {
        return $input;
    }

    return new A();
}

$a = foobaz(new AChild());
$b = foobaz(new B());