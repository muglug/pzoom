<?php
class B {
    /**
     * @param string|null $str
     * @return string
     */
    public function barBar($str) {
        if (empty($str)) {
            $str = "";
        }
        return $str;
    }
}
