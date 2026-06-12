<?php
class HelloWorld
{
    public function sayHello(): void
    {
        $a = explode("/", "a/b/c");
        $x = $a[-3];
        echo $x;
    }
}
