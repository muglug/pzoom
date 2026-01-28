<?php
interface I1 {
    /**
     * @param string $type
     * @return bool
     */
    public function takesString($type);
}

interface I2 extends I1 {
    public function takesString($type);
}

class C implements I2 {
    public function takesString($type) {
        return true;
    }
}
