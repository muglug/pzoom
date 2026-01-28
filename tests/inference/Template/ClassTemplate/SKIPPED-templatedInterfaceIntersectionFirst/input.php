<?php
/** @psalm-template T */
interface IParent {
    /** @psalm-return T */
    function foo();
}

interface IChild extends IParent {}

class C {}

/** @psalm-return IParent<C>&IChild */
function makeConcrete() : IChild {
    return new class() implements IChild {
        public function foo() {
            return new C();
        }
    };
}

$a = makeConcrete()->foo();