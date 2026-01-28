<?php
interface Logger {
    /** @return void */
    public function log(string $msg);
}

class Application {
    /** @var Logger|null */
    private $logger;

    /** @return void */
    public function setLogger(Logger $logger) {
         $this->logger = $logger;
    }
}

$app = new Application;
$app->setLogger(new class implements Logger {
    /** @return void */
    public function log(string $msg) {
        echo $msg;
    }
});
