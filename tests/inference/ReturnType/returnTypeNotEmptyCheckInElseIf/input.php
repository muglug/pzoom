<?php
class B {
    /**
     * @param string|null $str
     * @return string
     */
    public function barBar($str) {
        if ($str === "badger") {
            // do nothing
        }
        elseif (empty($str)) {
            $str = "";
        }
        return $str;
    }
}
