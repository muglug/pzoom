<?php
class A {
    /**
     * @psalm-suppress UndefinedClass
     * @psalm-suppress MixedMethodCall
     * @psalm-suppress MissingReturnType
     */
    public function b() {
        B::fooFoo()->barBar()->bat()->baz()->bam()->bas()->bee()->bet()->bes()->bis();
    }
}
