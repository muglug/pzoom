<?php
interface I {
    public string $value { set; }
}

class A implements I {
    private string $_value = "hello";

    public string $value = "default" {
        set => $this->_value = $value;
    }
}

function test(I $w): void {
    $w->value = "test";
}
