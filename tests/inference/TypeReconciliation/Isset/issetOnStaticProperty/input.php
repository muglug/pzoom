<?php
class Singleton {
    private static self $instance;
    public function getInstance(): self {
        if (isset(self::$instance)) {
            return self::$instance;
        }
        return self::$instance = new self();
    }
    private function __construct() {}
}
