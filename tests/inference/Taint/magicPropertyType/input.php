<?php
class Magic {
    private $params = [];

    public function __get(string $a) {
        return $this->params[$a];
    }

    public function __set(string $a, $value) {
        $this->params[$a] = $value;
    }
}

$m = new Magic();
$m->taint = $_GET["input"];
echo $m->taint;
