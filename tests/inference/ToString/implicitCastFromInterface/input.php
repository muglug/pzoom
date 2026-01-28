<?php
interface I {
    public function __toString();
}

function takesString(string $str): void { }

function takesI(I $i): void
{
    takesString($i);
}
