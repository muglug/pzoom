<?php
class Request {
    const SOURCE_GET = "GET";
    const SOURCE_POST = "POST";
    const SOURCE_BODY = "BODY";

    private function getBody() : string {
        return "";
    }

    /**
     * @template TSource as self::SOURCE_*
     * @param TSource $source
     * @return (TSource is "BODY" ? object|list : array)
     * @psalm-taint-source input
     */
    public function getParams(
        string $source = self::SOURCE_GET
    ) {
        if ($source === "GET") {
            return $_GET;
        }

        if ($source === "POST") {
            throw new \UnexpectedValueException("bad");
        }

        /** @psalm-suppress MixedAssignment */
        $decoded = json_decode($this->getBody(), false);

        if (!is_object($decoded) && !is_array($decoded)) {
            throw new \UnexpectedValueException("bad");
        }

        return $decoded;
    }
}

/** @psalm-suppress MixedArgument */
echo (new Request)->getParams()["a"];

/** @psalm-suppress MixedArgument */
echo (new Request)->getParams(Request::SOURCE_GET)["a"];
