<?php
class One {}

class B {
    /**
     * @return One|null
     */
    public function barBar() {
        $baz = rand(0,100) > 50 ? new One() : null;

        // should have no effect
        if ($baz === null) {
            $baz = null;
        }

        return $baz;
    }
}
