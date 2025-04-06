#let template(title: none) = body => {
  assert.ne(title, none, message: "`title` is not optional")

  align(center + horizon, title)
  pagebreak()
  body
}

#let helper(body) = {
  assert.eq(type(body), str, message: "`body` must be of type str")

  [Helper: #body]
}
