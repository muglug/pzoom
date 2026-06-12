<?php
class A1 {
}
class A2 {
}
class B {
    /**
     * @return A1
     */
    public function barBar(A1 $a1 = null, A2 $a2 = null) {
        if (!$a1) {
            throw new \Exception();
        }
        elseif (!$a2) {
            throw new \Exception();
        }
        return $a1;
    }
}
