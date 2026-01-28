<?php
interface I {
    /**
     * @param self $i
     * @return self
     */
    function foo(self $i) : self;
}

class C implements I {
    public function foo(I $i) : I {
        return $i;
    }
}

function takeI(I $i) : I {
    return (new C)->foo($i);
}
