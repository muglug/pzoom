<?php
class Example {
    const FOO = "foo";
    /**
     * @param array{bar:string, ...} $params
     */
    public function test(array $params) : bool {
        if (isset($params[self::FOO])) {
            return true;
        }

        if (isset($params["bat"])) {
            return true;
        }

        return false;
    }
}
