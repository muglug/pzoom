<?php
class Port {
   private int $portNumber;

   public function __construct(int $portNumber) {
      if (!$this->isValidPort($portNumber)) {
         throw new Exception();
      }

      $this->portNumber = $portNumber;
   }

   /**
    * @psalm-pure
    */
   private function isValidPort(int $portNumber): bool {
      return $portNumber >= 1 && $portNumber <= 1000;
   }
}
