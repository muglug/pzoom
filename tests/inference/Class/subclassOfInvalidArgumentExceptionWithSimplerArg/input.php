<?php
class A extends InvalidArgumentException {
    /**
     * @param string $message
     * @param int $code
     * @param Throwable|null $previous_exception
     */
    public function __construct($message, $code, $previous_exception) {
        parent::__construct($message, $code, $previous_exception);
    }
}
