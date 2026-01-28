<?php
class Example {
    const FOO = "foo";
    public function test() : bool {
        $params = ["bar" => "bat"];

        if (isset($params[self::FOO])) {
            return true;
        }

        return false;
    }
}
