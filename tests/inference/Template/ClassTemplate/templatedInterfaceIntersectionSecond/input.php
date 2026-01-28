<?php
/** @psalm-template T */
interface IParent {
    /** @psalm-return T */
    function foo();
}

/** @psalm-suppress MissingTemplateParam */
interface IChild extends IParent {}

class C {}

/** @psalm-return IChild&IParent<C> */
function makeConcrete() : IChild {
    return new class() implements IChild {
        public function foo() {
            return new C();
        }
    };
}

$a = makeConcrete()->foo();
