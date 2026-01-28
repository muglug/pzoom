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
    public static function collectStatic($elt, ...$elts) {
        $ret = $elt;
        foreach ($elts as $item) {
            if (rand(0, 1)) {
                $ret = $item;
            }
        }
        return $ret;
    }
}

echo A::collectStatic("a");