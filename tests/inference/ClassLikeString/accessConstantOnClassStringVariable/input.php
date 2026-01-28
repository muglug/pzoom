<?php
class Beep {
    /** @var string */
    public static $boop = "boop";
}
$beep = rand(0, 1) ? new Beep() : Beep::class;
echo $beep::$boop;
                
