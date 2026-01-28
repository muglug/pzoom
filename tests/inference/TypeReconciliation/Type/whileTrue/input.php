<?php
class One {
    /**
     * @return array|false
     */
    public function fooFoo(){
        return rand(0,100) ? ["hello"] : false;
    }

    /** @return void */
    public function barBar(){
        while ($row = $this->fooFoo()) {
            $row[0] = "bad";
        }
    }
}