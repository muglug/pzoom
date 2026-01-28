<?php

class A {
    /** @var mixed */
    public $_array_value = null;

    private function getArrayValue() : ?array {
        return rand(0, 1) ? [] : null;
    }

    public function setValue(string $var) : void {
        $this->_array_value = $this->getArrayValue();

        if ($this->_array_value !== null && !count($this->_array_value)) {
            return;
        }

        switch ($var) {
            case "a":
                foreach ($this->_array_value ?: [] as $v) {}
                break;

            case "b":
                foreach ($this->_array_value ?: [] as $v) {}
                break;
        }
    }
}