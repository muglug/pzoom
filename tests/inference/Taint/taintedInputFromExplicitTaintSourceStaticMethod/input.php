<?php
class Request {
    /**
     * @psalm-taint-source input
     */
    public static function getName() : string {
        return "";
    }
}


echo Request::getName();
