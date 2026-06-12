<?php
interface I {
    /**
     * @param self $f
     */
    public function foo(self $f) : self;
}

class C implements I {
    public function foo(I $f) : I {
        return new C();
    }
}
