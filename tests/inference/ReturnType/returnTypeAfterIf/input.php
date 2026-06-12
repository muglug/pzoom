<?php
class B {
    /**
     * @return string|null
     */
    public function barBar() {
        $str = null;
        $bar1 = rand(0, 100) > 40;
        if ($bar1) {
            $str = "";
        }
        return $str;
    }
}
