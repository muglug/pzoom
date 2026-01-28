<?php
class A {
    /**
     * @psalm-suppress UndefinedClass, MixedMethodCall,MissingReturnType because reasons
     */
    public function b() {
        B::fooFoo()->barBar()->bat()->baz()->bam()->bas()->bee()->bet()->bes()->bis();
    }
}
