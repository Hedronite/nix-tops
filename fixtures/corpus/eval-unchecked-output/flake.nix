{
  description = "Corpus gate-3 case: an output the evaluator does not check";
  outputs = { self }: {
    lib.double = n: n * 2;
    fooBar = { };
  };
}
