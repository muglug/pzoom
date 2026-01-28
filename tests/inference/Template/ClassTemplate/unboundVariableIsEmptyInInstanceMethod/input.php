<?php
class A {
    /**
     * @template TE
     * @template TR
     *
     * @param TE $elt
     * @param TR ...$elts
     *
     * @return TE|TR
     */
    public function collectInstance($elt, ...$elts) {
        $ret = $elt;
        foreach ($elts as $item) {
            if (rand(0, 1)) {
                $ret = $item;
            }
        }
        return $ret;
    }
}

echo (new A)->collectInstance("a");