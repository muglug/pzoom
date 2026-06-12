<?php
class B {
    /**
     * @param string|null $str
     * @return string
     */
    public function barBar($str) {
        if (!empty($str)) {
            // do nothing
        }
        else {
            $str = "";
        }
        return $str;
    }
}
