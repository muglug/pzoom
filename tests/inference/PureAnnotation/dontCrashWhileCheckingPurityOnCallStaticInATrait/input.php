<?php
/**
 * @method static static tt()
 */
trait Date {
    public static function __callStatic(string $_method, array $_parameters){
    }
}

class Date2{
    use Date;
}

Date2::tt();
                    
