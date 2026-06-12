<?php
interface I {
    public string $name { get; set; }
}

class A implements I {
    private string $_name = "hello";

    public string $name {
        get => $this->_name;
        set => $this->_name = $value;
    }
}

function test(I $rw): void {
    $rw->name = "test";
    echo $rw->name;
}
