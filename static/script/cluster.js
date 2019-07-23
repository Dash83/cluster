var current = undefined;
var viewing = undefined;
var descriptor = {};
var hosts = {};
var invocations = {};
var hostStates = {};

let Host = class {
  constructor(record) {
    this.id = record.id;
    this.hostname = record.hostname;
    this.state = record.state.desc;
    this.target = record.state.id;
    this.listing = undefined;
  }

  makeId() {
    var element = document.createElement("div");
    element.classList.add("id");
    element.appendChild(document.createTextNode(this.id));
    return element;
  }

  makeName() {
    var element = document.createElement("div");
    element.classList.add("hostname");
    element.appendChild(document.createTextNode(this.hostname));
    return element;
  }

  makeState() {
    var element = document.createElement("a");
    element.classList.add("state");
    element.classList.add(this.state);
    element.appendChild(document.createTextNode(this.state));
    if (this.target !== null) {
      var target = this.target;
      element.addEventListener("click", function() {
        viewInvocation(target);
      });
    }
    return element;
  }

  get element() {
    if (this.listing === undefined) {
      this.listing = document.createElement("div");
      this.listing.classList.add("host");
      this.listing.appendChild(this.makeId());
      this.listing.appendChild(this.makeName());
      this.listing.appendChild(this.makeState());
      this.listing.setAttribute("uuid", this.id);
    }
    return this.listing;
  }
};

let Invocation = class {
  constructor(record) {
    this.id = record.id;
    this.name = record.name;
    this.url = record.url;
    this.commit = record.commit;
    this.start = record.start;
    this.listing = undefined;
    if (this.name === null) {
      this.failed = true;
    } else {
      this.failed = false;
    }
  }

  makeId() {
    var element = document.createElement("div");
    element.classList.add("id");
    element.appendChild(document.createTextNode(this.id));
    return element;
  }

  makeName() {
    var element = document.createElement("div");
    element.classList.add("name");
    if (this.failed) {
      element.classList.add("unresolved");
      element.appendChild(document.createTextNode("(failed)")); 
    } else {
      element.appendChild(document.createTextNode(this.name));
    }
    return element;
  }

  makeUrl() {
    var element = document.createElement("a");
    element.classList.add("url");
    element.setAttribute("href", this.url);
    element.appendChild(document.createTextNode(this.url));
    return element;
  }

  makeCommit() {
    var element = document.createElement("div");
    element.classList.add("commit");
    element.appendChild(document.createTextNode(this.commit.substring(0, 10)));
    return element;
  }

  makeExpandButton() {
    var element = document.createElement("a");
    element.classList.add("popout");
    element.classList.add("button");
    element.appendChild(materialIcon("open_in_new"));
    var id = this.id;
    element.addEventListener("click", function() {
      viewInvocation(id);
    });
    return element;
  }

  makeTime() {
    var element = document.createElement("div");
    element.classList.add("time");
    element.appendChild(document.createTextNode(formatDate(new Date(this.start))));
    return element;
  }

  makeStatus() {
    var element;
    if (this.failed) {
      element = materialIcon("clear");
    } else {
      element = materialIcon("check");
      element.classList.add("ok");
    }
    element.classList.add("status");
    return element;
  }

  get element() {
    if (this.listing === undefined) {
      this.listing = document.createElement("div");
      this.listing.classList.add("invocation");
      this.listing.appendChild(this.makeId());
      this.listing.appendChild(this.makeName());
      this.listing.appendChild(this.makeUrl());
      this.listing.appendChild(this.makeCommit());
      if (!this.failed) {
        this.listing.appendChild(this.makeExpandButton());
      }
      this.listing.appendChild(this.makeTime());
      this.listing.appendChild(this.makeStatus());
      this.listing.setAttribute("uuid", this.id);
    }
    return this.listing;
  }
};

function get(url, callback, err) {
  var xhttp = new XMLHttpRequest();
  xhttp.open("GET", url);
  xhttp.send();
  xhttp.onreadystatechange = (e) => {
    var response;
    try {
      response = JSON.parse(xhttp.responseText);
    } catch (e) { return; }
    if (response.status == "ok") {
      delete response.status;
      if ('payload' in response) {
        callback(response.payload);
      } else {
        callback();
      }
    } else {
      if (!('msg' in response)) {
        response.msg = "an error occured";
      }
      err(response.msg);
    }
  }
}

function pad(string) {
  string = "" + string;
  if (string.length == 1) {
    return "0" + string;
  } else {
    return string;
  }
}

function formatDate(date) {
  return pad(date.getHours()) + ":" +
      pad(date.getMinutes()) + ":" +
      pad(date.getSeconds()) + " " +
      date.getDate() + "/" +
      (date.getMonth() + 1) + "/" +
      date.getFullYear();
}

function materialIcon(name) {
  var element = document.createElement("i");
  element.classList.add("material-icons");
  element.appendChild(document.createTextNode(name));
  return element;
}

function makeEmpty() {
  var element = document.createElement("div");
  element.classList.add("invocation");
  var p = document.createElement("p");
  p.classList.add("placeholder");
  p.appendChild(document.createTextNode("no active invocation"));
  element.appendChild(p);
  return element;
}

function updateCurrent() {
  var active = document.getElementById("active");
  get("/api/current", function(id) {
    if (current !== id) {
      current = id;
      updateInvocations(function() {
        while (active.firstChild) {
          active.removeChild(active.firstChild);
        }
        active.appendChild(invocations[current].element);
      });
    }
  }, function(err) {
    current = undefined;
    updateInvocations(function() {
      while (active.firstChild) {
        active.removeChild(active.firstChild);
      }
      active.appendChild(makeEmpty());
    });
  });
}

function updateInvocations(callback) {
  get("/api/invocations", function(response) {
    for (record of response) {
      if (!(record.id in invocations)) {
        invocations[record.id] = new Invocation(record);
      }
    }
    for (id in invocations) {
      var index = response.findIndex(function(record) {
        return record.id == id;
      });
      if (index === -1) {
        delete invocations[id];
      }
    }
    var list = document.getElementById("invocations");
    var children = [];
    if (current !== undefined) {
      children.push(current);
    }
    for (child of list.children) {
      var id = child.getAttribute("uuid");
      children.push(id);
      if (id === current || !(id in invocations)) {
        list.removeChild(child);
      }
    }
    var placeholder = document.getElementById("history_placeholder");
    if (list.children.length == 0) {
      placeholder.classList.remove("hidden");
    } else {
      placeholder.classList.add("hidden");
    }
    for (id in invocations) {
      if (!children.includes(id)) {
        list.appendChild(invocations[id].element);
      }
    }
    if (viewing !== undefined && !children.includes(viewing)
        && (current === undefined || current !== viewing)) {
      viewing = undefined;
      var content = document.getElementById("content");
      while (content.firstChild) {
        content.removeChild(content.firstChild);
      }
      document.getElementById("center_placeholder").classList.remove("hidden");
    }
    callback();
  }, function(err) {});
}

function updateHosts() {
  get("/api/hosts", function(response) {
    var list = document.getElementById("hosts");
    while (list.firstChild) {
      list.removeChild(list.firstChild);
    }
    for (record of response) {
      list.appendChild((new Host(record)).element);
      hosts[record.hostname] = record;
      updateHostState(record.hostname);
    }
    var placeholder = document.getElementById("hosts_placeholder");
    if (list.children.length == 0) {
      placeholder.classList.remove("hidden");
    } else {
      placeholder.classList.add("hidden");
    }
  }, function(err) {});
}

function updateHostState(host) {
  if (host in hostStates) {
    var element = hostStates[host];
    while (element.firstChild) {
      element.removeChild(element.firstChild);
    }
    element.classList = [];
    element.classList.add("state");
    element.removeAttribute("href");
    if (host in hosts) {
      if (hosts[host].hostname in descriptor.logs) {
        element.classList.add("logs");
        element.appendChild(document.createTextNode("logs"));
        element.setAttribute("href", descriptor.logs[hosts[host].hostname]);
      } else if (hosts[host].state.id === viewing) {
        element.classList.add(hosts[host].state.desc);
        element.appendChild(document.createTextNode(hosts[host].state.desc));
      } else if (!('id' in hosts[host].state)) {
        element.classList.add(hosts[host].state.desc);
        element.appendChild(document.createTextNode(hosts[host].state.desc));
      } else if (viewing === current) {
        element.classList.add("busy");
        element.appendChild(document.createTextNode("busy"));
      } else {
        element.classList.add("abandoned");
        element.appendChild(document.createTextNode("abandoned"));
      }
    } else {
      element.classList.add("disconnected");
      element.appendChild(document.createTextNode("disconnected"));
    }
  }
}

function renderInvocation(invocation) {
  descriptor = invocation;
  hostStates = {};
  document.getElementById("center_placeholder").classList.add("hidden");
  var content = document.getElementById("content");
  while (content.firstChild) {
    content.removeChild(content.firstChild);
  }
  var name = document.createElement("h2");
  name.appendChild(document.createTextNode(invocation.descriptor.name));
  content.appendChild(name);
  var start = document.createElement("div");
  start.classList.add("time");
  var time = document.createTextNode(formatDate(new Date(invocation.start)));
  start.appendChild(time);
  content.appendChild(start);
  var id = document.createElement("div");
  id.classList.add("id");
  id.appendChild(document.createTextNode(invocation.id));
  content.appendChild(id);
  var repo = document.createElement("div");
  repo.classList.add("repo");
  var url = document.createElement("a");
  url.classList.add("url");
  url.setAttribute("href", invocation.url);
  url.appendChild(document.createTextNode(invocation.url));
  repo.appendChild(url);
  var commit = document.createElement("div");
  var hash = document.createElement("a");
  hash.classList.add("commit");
  hash.appendChild(document.createTextNode(invocation.commit));
  commit.appendChild(hash);
  repo.appendChild(commit);
  content.appendChild(repo);
  var reinvoke = document.createElement("a");
  reinvoke.id = "reinvoke";
  reinvoke.classList.add("text_button");
  reinvoke.appendChild(document.createTextNode("reinvoke"));
  reinvoke.addEventListener("click", function() {
    get("/api/reinvoke/" + invocation.id, function(response) {
      updateCurrent();
      viewing = response.id;
      renderInvocation(response);
    }, function(err) {
      // TODO toast or something
    })
  });
  content.appendChild(reinvoke);
  if (invocation.id === current) {
    var cancel = document.createElement("a");
    cancel.id = "cancel";
    cancel.classList.add("text_button");
    cancel.appendChild(document.createTextNode("cancel"));
    cancel.addEventListener("click", function() {
      get("/api/cancel", function() {
        updateCurrent();
        content.removeChild(cancel); 
      }, function(err) {
        // TODO toast or something
      });
    });
    content.appendChild(cancel);
  }
  var setup = document.createElement("div");
  setup.id = "setup";
  if (invocation.descriptor.command !== null) {
    var global = document.createElement("h3");
    global.appendChild(document.createTextNode("global setup"));
    setup.appendChild(global);
    setup.appendChild(makeCommand(invocation.descriptor.command, invocation.descriptor.args));
  }
  var hostHeader = document.createElement("h3");
  hostHeader.appendChild(document.createTextNode("hosts"));
  setup.appendChild(hostHeader);
  for (host in invocation.descriptor.hosts) {
    var hostname = document.createElement("p");
    hostname.classList.add("hostname");
    hostname.appendChild(document.createTextNode(host));
    var state = document.createElement("a");
    hostStates[host] = state;
    updateHostState(host);
    hostname.appendChild(state);
    setup.appendChild(hostname);
    var record = invocation.descriptor.hosts[host];
    if (record.command !== null) {
      setup.appendChild(makeCommand(record.command, record.args)); 
    }
  }
  if (invocation.descriptor.gen_logs) {
    var note = document.createElement("p");
    note.appendChild(document.createTextNode("logs files will be generated from standard output"));
    setup.appendChild(note);
  }
  var logDir = document.createElement("p");
  logDir.appendChild(document.createTextNode("log files on hosts will be uploaded from "));
  var dir = document.createElement("code");
  dir.appendChild(document.createTextNode(invocation.descriptor.log_dir));
  logDir.appendChild(dir);
  setup.appendChild(logDir);
  content.appendChild(setup);
}

function makeCommand(command, args) {
  for (arg of args) {
    if (arg.includes(" ")) {
      if (!arg.includes("\"")) {
        arg = "\"" + arg + "\"";
      } else {
        arg = "\'" + arg + "\'";
      }
    }
    command += " " + arg;
  }
  var element = document.createElement("pre");
  element.appendChild(document.createTextNode(command));
  return element;
}

function viewInvocation(id) {
  get("/api/invocation/" + id, function(response) {
    viewing = id;
    renderInvocation(response);
  }, function(err) {});
}

function updateViewing() {
  if (viewing !== undefined) {
    viewInvocation(viewing);
  }
}

setInterval(updateCurrent, 500);
setInterval(updateHosts, 500);
setInterval(updateViewing, 500);

document.addEventListener('DOMContentLoaded', function() {
  updateCurrent();
  updateHosts();
  document.getElementById("invoke_button").addEventListener("click", function() {
    var url = encodeURIComponent(document.getElementById('input').value).trim();
    if (url.length > 0) {
      document.getElementById('input').value = '';
      get("/api/invoke/" + url, function(response) {
        viewing = response.id;
        renderInvocation(response);
      }, function(err) {
        // TODO toast or something
      });
    }
  });
}, false);
